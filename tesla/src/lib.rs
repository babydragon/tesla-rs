use std::collections::HashMap;

use http::{StatusCode, HeaderMap};
pub use reqwest;
use reqwest::{Client, Url};
use reqwest::header;
use reqwest::redirect::Policy;
use serde::de::DeserializeOwned;

pub use models::*;
pub use tesla_rs_error::*;
use rand::Rng;
use sha2::{Sha256, Digest};
use select::document::Document;
use select::predicate::{Attr, Name, And};
use crate::reqwest::header::HeaderValue;
use std::borrow::Borrow;

use async_recursion::async_recursion;

mod tesla_rs_error;
mod models;

const DEFAULT_BASE_URI: &str = "https://owner-api.teslamotors.com/api/1/";
const ENDPOINT_GET_VEHICLES: &str = "vehicles";
#[allow(dead_code)]
const ENDPOINT_GET_VEHICLE: &str = "vehicles/{}";

const VEHICLE_CHARGE_STATE: &str = "data_request/charge_state";
const VEHICLE_GUI_SETTINGS: &str = "data_request/gui_settings";
const VEHICLE_DATA: &str = "vehicle_data";

const VEHICLE_COMMAND_WAKE: &str = "wake_up";
const VEHICLE_COMMAND_FLASH: &str = "flash_lights";
const VEHICLE_COMMAND_DOOR_UNLOCK: &str = "door_unlock";
const VEHICLE_COMMAND_DOOR_LOCK: &str = "door_lock";
const VEHICLE_COMMAND_HONK_HORN: &str = "honk_horn";
const VEHICLE_COMMAND_AUTO_CONDITIONING_START: &str = "auto_conditioning_start";
const VEHICLE_COMMAND_AUTO_CONDITIONING_STOP: &str = "auto_conditioning_stop";
const VEHICLE_COMMAND_REMOTE_START_DRIVE: &str = "remote_start_drive";
const VEHICLE_COMMAND_CHARGE_PORT_DOOR_OPEN: &str = "charge_port_door_open";
const VEHICLE_COMMAND_CHARGE_PORT_DOOR_CLOSE: &str = "charge_port_door_close";

// We expect here because this is parsing a const and will not fail
macro_rules! endpoint_url {
    ($client: ident, $e:expr) => {
        $client.get_base_url().join($e).expect("cannot parse endpoint")
    }
}

#[derive(Clone)]
pub struct TeslaClient {
    pub api_root: reqwest::Url,
    client: Client,
}

#[derive(Clone)]
pub struct VehicleClient {
    tesla_client: TeslaClient,
    vehicle_id: u64,
}

impl TeslaClient {
    pub async fn authenticate(email: &str, password: &str) -> Result<String, TeslaError> {
        TeslaClient::authenticate_using_api_root(DEFAULT_BASE_URI, email, password).await
    }

    pub async fn authenticate_using_api_root(api_root: &str, email: &str, password: &str) -> Result<String, TeslaError> {
        let resp = TeslaClient::call_auth_route(api_root, email, password).await?;

        let expires_in_days = resp.expires_in / 60 / 60 / 24;
        println!("The access token will expire in {} days", expires_in_days);
        Ok(resp.access_token)
    }

    async fn call_auth_route(api_root: &str, email: &str, password: &str) -> Result<AuthResponse, TeslaError> {
        let auth_endpoint = "https://auth.tesla.com/oauth2/v3/authorize";

        let policy = Policy::custom(|attempt| {
            dbg!("redirect to {}", attempt.url());
            if attempt.url().path() == "/void/callback" {
                attempt.stop()
            } else {
                attempt.follow()
            }
        });
        let client = Client::builder().cookie_store(true).redirect(policy).build().expect("Fail to build auth client");

        // step 1 get cookie and hidden form field
        let code_verifier: String = rand::thread_rng().sample_iter(rand::distributions::Alphanumeric).take(86).map(char::from).collect();
        let mut hasher = Sha256::new();
        hasher.update(code_verifier.clone());
        let code_challenge = format!("{:x}", hasher.finalize());

        let state: String = rand::thread_rng().sample_iter(rand::distributions::Alphanumeric).take(16).map(char::from).collect();
        let mut query_map = HashMap::new();
        query_map.insert("client_id", "ownerapi");
        query_map.insert("code_challenge", code_challenge.as_str());
        query_map.insert("code_challenge_method", "S256");
        query_map.insert("redirect_uri", "https://auth.tesla.com/void/callback");
        query_map.insert("response_type", "code");
        query_map.insert("scope", "openid email offline_access");
        query_map.insert("state", state.as_str());
        query_map.insert("login_hint", email);

        let mut url = reqwest::Url::parse(auth_endpoint).expect("Could not parse API URL");
        url.query_pairs_mut().extend_pairs(query_map.iter());
        let response = client.get(url).send().await?;

        let body = response.text().await?;
        dbg!("{}", body.clone());

        // step 2 post to get token
        query_map = HashMap::new();
        query_map.insert("client_id", "ownerapi");
        query_map.insert("code_challenge", code_challenge.as_str());
        query_map.insert("code_challenge_method", "S256");
        query_map.insert("redirect_uri", "https://auth.tesla.com/void/callback");
        query_map.insert("response_type", "code");
        query_map.insert("scope", "openid email offline_access");
        query_map.insert("state", state.as_str());

        dbg!("====  start to post  ====");

        let mut post_url = reqwest::Url::parse(auth_endpoint).expect("Could not parse API URL");
        post_url.query_pairs_mut().clear().extend_pairs(query_map.iter());

        let code = TeslaClient::try_post_to_fetch_token(post_url, body.as_str(), email, password, &client).await?;
        dbg!("code: {}", code.as_str());

        // step 3

        let mut oauth_token_params = HashMap::new();
        oauth_token_params.insert("grant_type", "authorization_code");
        oauth_token_params.insert("client_id", "ownerapi");
        oauth_token_params.insert("code", code.as_str());
        oauth_token_params.insert("code_verifier", code_verifier.as_str());
        oauth_token_params.insert("redirect_uri", "https://auth.tesla.com/void/callback");

        // FIXME: when to use .cn when to use .com ?
        let oauth_token_url = reqwest::Url::parse("https://auth.tesla.cn/oauth2/v3/token").expect("Could not parse oauth token URL");
        let oauth_response = client.post(oauth_token_url).json(&oauth_token_params).send().await?;

        let oauth_token = if oauth_response.status().is_success() {
            let oauth_token = oauth_response.json::<OAuthToken>().await?;
            dbg!("oauth response content {}", &oauth_token);
            oauth_token
        } else {
            dbg!("oauth response fail,  content {}", oauth_response.text().await?);
            return Err(TeslaError::AuthError);
        };

        dbg!("oauth token fetch success {}", &oauth_token);

        // step 4
        let mut sso_token_params = HashMap::new();
        sso_token_params.insert("grant_type", "urn:ietf:params:oauth:grant-type:jwt-bearer");
        sso_token_params.insert("client_id", "81527cff06843c8634fdc09e8ac0abefb46ac849f38fe1e431c2ef2106796384");
        sso_token_params.insert("client_secret", "c7257eb71a564034f9419ee651c7d0e5f7aa6bfbd18bafb5c5c033b093bb2fa3");

        let sso_token_url = reqwest::Url::parse("https://owner-api.teslamotors.com/oauth/token").expect("Could not parse sso token URL");
        let sso_response = client.post(sso_token_url).bearer_auth(oauth_token.access_token).json(&sso_token_params).send().await?.json::<AuthResponse>().await?;

        dbg!("sso token fetch success {}", &sso_response);

        Ok(sso_response)
    }

    #[async_recursion(?Send)]
    async fn try_post_to_fetch_token(url: Url, body: &str, email: &str, password: &str, client: &reqwest::Client) -> Result<String, TeslaError> {
        // parse response text
        let document = Document::from(body);
        let mut form_values: HashMap<&str, &str> = document.find(And(Name("input"), Attr("type", "hidden")))
            .map(|e| (e.attr("name").unwrap(), e.attr("value").unwrap())).collect();
        form_values.insert("identity", email);
        form_values.insert("credential", password);

        let resp = client.post(url).form(&form_values).send().await?;

        if resp.status().is_redirection() {
            dbg!("post redirection to callback, try to get code from redirect url");
            match resp.headers().get("location") {
                None => {
                    Err(TeslaError::AuthError)
                }
                Some(location) => {
                    let location_str = location.to_str().unwrap();
                    let redirect_url = reqwest::Url::parse(location_str).expect("Fail to parse auth code location");
                    let code = redirect_url.query_pairs().find(|q| q.0 == "code").expect("Fail to find code parameter").1;
                    Ok(code.to_string())
                }
            }
        } else {
            // still redirect to a login page
            dbg!("post redirection to login page, try post again");
            let final_url = resp.url().clone();
            let post_resp_body = resp.text().await?;
            TeslaClient::try_post_to_fetch_token(final_url, post_resp_body.as_str(), email, password, client).await
        }
    }

    pub fn default(access_token: &str) -> TeslaClient {
        TeslaClient::new(DEFAULT_BASE_URI, access_token)
    }

    pub fn new(api_root: &str, access_token: &str) -> TeslaClient {
        let mut headers = header::HeaderMap::new();

        let auth_value = header::HeaderValue::from_str(format!("Bearer {}", access_token).as_str()).unwrap();

        headers.insert(header::AUTHORIZATION, auth_value);

        let client = Client::builder()
            .default_headers(headers)
            .build()
            .expect("Could not create client");

        TeslaClient {
            api_root: reqwest::Url::parse(api_root).expect("Could not parse API root"),
            client,
        }
    }

    pub fn vehicle(&self, vehicle_id: u64) -> VehicleClient {
        VehicleClient {
            tesla_client: self.clone(),
            vehicle_id,
        }
    }

    pub async fn get_vehicles(&self) -> Result<Vec<Vehicle>, TeslaError> {
        let url = endpoint_url!(self, ENDPOINT_GET_VEHICLES);
        let response = self.client.get(url).send().await?;
        if response.status() == 200 {
            let vehicle_response: ResponseArray<Vehicle> = response.json().await?;
            Ok(vehicle_response.into_response())
        } else {
            Err(self.get_error_from_response(response))
        }
    }

    pub async fn get_vehicle_by_name(&self, name: &str) -> Result<Option<Vehicle>, TeslaError> {
        let vehicle = self.get_vehicles().await?.into_iter()
            .find(|v| v.display_name.to_lowercase() == name.to_lowercase());

        Ok(vehicle)
    }

    fn get_base_url(&self) -> reqwest::Url {
        self.api_root.clone()
    }

    fn get_error_from_response(&self, response: reqwest::Response) -> TeslaError {
        let headers = response.headers();
        let mut err = TeslaError::ParseAppError(AppError {
            message: "Unspecified error".to_owned()
        });
        if response.status() == 401 {
            let header_value = headers.get("www-authenticate");
            if header_value.is_some() {
                if header_value.unwrap().to_str().unwrap_or("").contains("invalid_token") {
                    err = TeslaError::InvalidTokenError;
                }
            }
        } else if response.status() == 404 {
            err = TeslaError::ParseAppError(AppError {
                message: "Not found error (404)".to_owned()
            });
        }
        err
    }
}

impl VehicleClient {
    pub async fn wake_up(&self) -> Result<Vehicle, TeslaError> {
        let url = endpoint_url!(self, VEHICLE_COMMAND_WAKE);

        let response = self.tesla_client.client.post(url).send().await?;
        if response.status() == 200 {
            let resp: Response<Vehicle> = response.json().await?;
            Ok(resp.into_response())
        } else {
            Err(self.tesla_client.get_error_from_response(response))
        }
    }

    pub async fn flash_lights(&self) -> Result<SimpleResponse, TeslaError> {
        self.post_simple_command(VEHICLE_COMMAND_FLASH).await
    }

    pub async fn door_unlock(&self) -> Result<SimpleResponse, TeslaError> {
        self.post_simple_command(VEHICLE_COMMAND_DOOR_UNLOCK).await
    }

    pub async fn door_lock(&self) -> Result<SimpleResponse, TeslaError> {
        self.post_simple_command(VEHICLE_COMMAND_DOOR_LOCK).await
    }

    pub async fn honk_horn(&self) -> Result<SimpleResponse, TeslaError> {
        self.post_simple_command(VEHICLE_COMMAND_HONK_HORN).await
    }

    pub async fn auto_conditioning_start(&self) -> Result<SimpleResponse, TeslaError> {
        self.post_simple_command(VEHICLE_COMMAND_AUTO_CONDITIONING_START).await
    }

    pub async fn auto_conditioning_stop(&self) -> Result<SimpleResponse, TeslaError> {
        self.post_simple_command(VEHICLE_COMMAND_AUTO_CONDITIONING_STOP).await
    }

    pub async fn remote_start_drive(&self) -> Result<SimpleResponse, TeslaError> {
        // TODO : Need to pass the password in the querystring
        let url = self.get_command_url(VEHICLE_COMMAND_REMOTE_START_DRIVE);
        let response = self.tesla_client.client.post(url).send().await?;
        if response.status() == 200 {
            let resp: Response<SimpleResponse> = response.json().await?;
            Ok(resp.into_response())
        } else {
            Err(self.tesla_client.get_error_from_response(response))
        }
    }

    pub async fn charge_port_door_open(&self) -> Result<SimpleResponse, TeslaError> {
        self.post_simple_command(VEHICLE_COMMAND_CHARGE_PORT_DOOR_OPEN).await
    }

    pub async fn charge_port_door_close(&self) -> Result<SimpleResponse, TeslaError> {
        self.post_simple_command(VEHICLE_COMMAND_CHARGE_PORT_DOOR_CLOSE).await
    }

    async fn post_simple_command(&self, command: &str) -> Result<SimpleResponse, TeslaError> {
        let url = self.get_command_url(command);
        let response = self.tesla_client.client.post(url).send().await?;
        if response.status() == 200 {
            let resp: Response<SimpleResponse> = response.json().await?;
            Ok(resp.into_response())
        } else {
            Err(self.tesla_client.get_error_from_response(response))
        }
    }

    pub async fn get(&self) -> Result<Vehicle, TeslaError> {
        let url = self.get_base_url();
        self.get_some_data(url).await
    }

    pub async fn get_all_data(&self) -> Result<FullVehicleData, TeslaError> {
        let url = endpoint_url!(self, VEHICLE_DATA);
        self.get_some_data(url).await
    }

    pub async fn get_soc(&self) -> Result<StateOfCharge, TeslaError> {
        let url = endpoint_url!(self, VEHICLE_CHARGE_STATE);
        self.get_some_data(url).await
    }

    pub async fn get_gui_settings(&self) -> Result<GuiSettings, TeslaError> {
        let url = endpoint_url!(self, VEHICLE_GUI_SETTINGS);
        self.get_some_data(url).await
    }

    async fn get_some_data<T: DeserializeOwned>(&self, url: reqwest::Url) -> Result<T, TeslaError> {
        let response = self.tesla_client.client.get(url).send().await?;
        if response.status() == 200 {
            let resp: Response<T> = response.json().await?;
            Ok(resp.into_response())
        } else {
            Err(self.tesla_client.get_error_from_response(response))
        }
    }

    fn get_base_url(&self) -> reqwest::Url {
        let vehicle_path = format!("vehicles/{}/", self.vehicle_id);

        self.tesla_client.api_root
            .join(vehicle_path.as_str())
            .unwrap()
    }

    fn get_command_url(&self, command: &str) -> reqwest::Url {
        let command_path = format!("vehicles/{}/command/{}", self.vehicle_id, command);

        self.tesla_client.api_root
            .join(command_path.as_str())
            .unwrap()
    }
}


#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
