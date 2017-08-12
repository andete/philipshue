use std::str::FromStr;
use std::collections::BTreeMap;

use serde_json::{self, to_vec};
use hyper;
use futures;
use futures::{future, Future};

use errors::{Result, HueError};
use ::hue::*;
use ::json::*;
use ::network::{self, HueFuture};

/// Attempts to discover bridges using `https://www.meethue.com/api/nupnp`
#[cfg(feature = "nupnp")]
pub fn discover() -> Result<Vec<Discovery>> {
    let (mut core, client) = network::make_core_and_tls_client();
    let uri = hyper::Uri::from_str("https://www.meethue.com/api/nupnp").unwrap();
    let req = hyper::Request::new(hyper::Get, uri);
    let future = client.request(req)
        .from_err::<HueError>()
        .and_then(network::body_from_res)
        .and_then(|body| {
            let r:Result<Vec<Discovery>> = serde_json::from_str(&body)
                .map_err(From::from);
            futures::done(r)
        });
    core.run(future)
}
/// Discovers bridge IP using UPnP
///
/// Waits for about 5 seconds to make sure it gets a response
#[cfg(feature = "ssdp")]
pub fn discover_upnp() -> ::std::result::Result<Vec<String>, ::ssdp::SSDPError> {
    use ssdp::header::{HeaderMut, Man, MX, ST};
    use ssdp::message::SearchRequest;
    use ssdp::FieldMap;
    use ssdp::message::Multicast;

    let mut request = SearchRequest::new();

    request.set(Man);
    request.set(MX(5));
    request.set(ST::Target(FieldMap::upnp("IpBridge")));

    request.multicast().map(|r| {
        r.into_iter()
            .map(|(_, src)| src.ip().to_string())
            .collect()
    })
}
/// Tries to register a user, returning the username if successful
///
/// This usually returns a `HueError::BridgeError` saying the link button needs to be pressed.
/// Therefore it recommended to call this function in a loop:
/// ## Example
/// ```no_run
/// use philipshue::errors::{HueError, HueErrorKind, BridgeError};
/// use philipshue::bridge::{self, Bridge};
/// use philipshue::network::Core;
///
/// let mut bridge = None;
/// // Discover a bridge
/// let bridge_ip = philipshue::bridge::discover().unwrap().pop().unwrap().into_ip();
/// let devicetype = "my_hue_app#homepc";
///
/// // Keep trying to register a user
/// loop{
///     match bridge::register_user(&bridge_ip, devicetype){
///         // A new user has succesfully been registered and the username is returned
///         Ok(username) => {
///             let core = Core::new().unwrap();
///             bridge = Some(Bridge::new(&core, bridge_ip, username));
///             break;
///         },
///         // Prompt the user to press the link button
///         Err(HueError(HueErrorKind::BridgeError{error: BridgeError::LinkButtonNotPressed, ..}, _)) => {
///             println!("Please, press the link on the bridge. Retrying in 5 seconds");
///             std::thread::sleep(std::time::Duration::from_secs(5));
///         },
///         // Some other error happened
///         Err(e) => {
///             println!("Unexpected error occured: {:?}", e);
///             break
///         }
///     }
/// }
/// ```
pub fn register_user(ip: &str, devicetype: &str) -> Result<String> {
    let mut core = network::Core::new().unwrap();
    let client = hyper::Client::new(&core.handle());
    
    let url = format!("http://{}/api", ip);
    let uri = hyper::Uri::from_str(&url).unwrap();
    let mut req = hyper::Request::new(hyper::Post, uri);
    let body = format!("{{\"devicetype\": {:?}}}", devicetype);
    req.set_body(body);
    let future = client.request(req)
        .from_err::<HueError>()
        .and_then(network::body_from_res)
        .and_then(|body| {
            let r:Result<Vec<HueResponse<User>>> = serde_json::from_str(&body)
                .map_err(From::from);
            futures::done(r)
        }).and_then(|mut r| {
            let username = r.pop().unwrap().into_result().map(|u| u.username);
            futures::done(username)
        });
    core.run(future)
}

#[derive(Debug)]
/// The bridge connection
pub struct Bridge {
    client: network::Client,
    url: String,
}

#[test]
fn get_ip_and_username() {
    let core = network::Core::new().unwrap();
    let b = Bridge::new(&core, "test", "hello");
    assert_eq!(b.get_ip(), "test");
    assert_eq!(b.get_username(), "hello");
}

/// Many commands on the bridge return an array of things that were succesful.
/// This is a type alias for that type.
pub type SuccessVec = Vec<JsonMap<String, JsonValue>>;

use serde::Deserialize;

fn extract<'de, T>(responses: Vec<HueResponse<T>>) -> Result<Vec<T>>
    where T: Deserialize<'de>
{
    let mut res_v = Vec::with_capacity(responses.len());
    for val in responses {
        res_v.push(val.into_result()?)
    }
    Ok(res_v)
}

impl Bridge {

    fn send<'a, T>(&self, req: hyper::Request) -> HueFuture<'a, T>
        where for<'de> T: Deserialize<'de>, T: 'a
    {
        let f = self.client.request(req)
            .from_err::<HueError>()
            .and_then(network::body_from_res)
            .and_then(|body| {
                let r:Result<T> = serde_json::from_str(&body)
                    .map_err(From::from);
                let r2 = match r {
                    Ok(r) => Ok(r),
                    Err(e1) => {
                        let e:Result<Vec<HueResponse<T>>> = serde_json::from_str(&body).map_err(From::from);
                        if let Ok(v) = e {
                            v.into_iter()
                                .next()
                                .ok_or_else(|| "Malformed response".into())
                                .and_then(HueResponse::into_result)
                        } else {
                            Err(e1)
                        }
                    },
                };
                futures::done(r2)
            });
        Box::new(f)
    }
    
    fn send_and_extract<'a, T>(&self, req: hyper::Request) -> HueFuture<'a, Vec<T>>
        where for<'de> T: Deserialize<'de>, T: 'a
    {
        let future = self.send(req)
            .and_then(|res| future::done(extract(res)));
        Box::new(future)
    }
    
    /// Creates a `Bridge` on the given IP with the given username
    pub fn new<S: Into<String>, U: Into<String>>(core:&network::Core, ip: S, username: U) -> Self {
        let client = network::Client::new(&core.handle());
        Bridge {
            client: client,
            url: format!("http://{}/api/{}/", ip.into(), username.into()),
        }
    }
    /// Gets the IP of bridge
    pub fn get_ip(&self) -> &str {
        self.url.split('/').nth(2).unwrap()
    }
    /// Gets the username this `Bridge` uses
    pub fn get_username(&self) -> &str {
        self.url.split('/').nth(4).unwrap()
    }
    /// Gets all lights that are connected to the bridge
    pub fn get_all_lights(&self) -> HueFuture<BTreeMap<usize, Light>> {
        
        let uri = hyper::Uri::from_str(&format!("{}lights", self.url)).unwrap();
        let req = hyper::Request::new(hyper::Get, uri);
        self.send(req)
    }
    /// Gets the light with the specific id
    pub fn get_light(&self, id: usize) -> HueFuture<Light> {
        let uri = hyper::Uri::from_str(&format!("{}lights/{}", self.url, id)).unwrap();
        let req = hyper::Request::new(hyper::Get, uri);
        self.send(req)
    }
    /// Gets all the light that were found last time a search for new lights was done
    pub fn get_new_lights(&self) -> HueFuture<BTreeMap<usize, Light>> {
        // TODO return lastscan too
        let uri = hyper::Uri::from_str(&format!("{}lights/new", self.url)).unwrap();
        let req = hyper::Request::new(hyper::Get, uri);
        self.send(req)
    }
    /// Makes the bridge search for new lights (and switches).
    ///
    /// The found lights can be retrieved with `get_new_lights()`
    pub fn search_for_new_lights(&self) -> HueFuture<SuccessVec> {
        // TODO Allow deviceids to be specified
        let uri = hyper::Uri::from_str(&format!("{}lights", self.url)).unwrap();
        let req = hyper::Request::new(hyper::Post, uri);
        self.send_and_extract(req)
    }
    /// Sets the state of a light by sending a `LightCommand` to the bridge for this light
    pub fn set_light_state(&self, id: usize, command: &LightCommand) -> HueFuture<SuccessVec> {
        let uri = hyper::Uri::from_str(&format!("{}lights/{}/state", self.url, id)).unwrap();
        let mut req = hyper::Request::new(hyper::Put, uri);
        let body = to_vec(command).map_err(From::from);
        let future = futures::done(body).and_then(move |body| {
            req.set_body(body);
            self.send_and_extract(req)
        });
        Box::new(future)
    }
    /// Renames the light
    pub fn rename_light(&self, id: usize, name: String) -> HueFuture<SuccessVec> {
        let mut name_map = BTreeMap::new();
        name_map.insert("name".to_owned(), name);
        let uri = hyper::Uri::from_str(&format!("{}lights/{}", self.url, id)).unwrap();
        let mut req = hyper::Request::new(hyper::Put, uri);
        let body = to_vec(&name_map).map_err(From::from);
        let future = futures::done(body).and_then(move |body| {
            req.set_body(body);
            self.send_and_extract(req)
        });
        Box::new(future)
    }
    /// Deletes a light from the bridge
    pub fn delete_light(&self, id: usize) -> HueFuture<SuccessVec> {
        let uri = hyper::Uri::from_str(&format!("{}lights/{}", self.url, id)).unwrap();
        let req = hyper::Request::new(hyper::Delete, uri);
        self.send_and_extract(req)
    }

    // GROUPS

    /// Gets all groups of the bridge
    pub fn get_all_groups(&self) -> HueFuture<BTreeMap<usize, Group>> {
        let uri = hyper::Uri::from_str(&format!("{}groups", self.url)).unwrap();
        let req = hyper::Request::new(hyper::Get, uri);
        self.send(req)
    }
    /// Creates a group and returns the ID of the group
    pub fn create_group(&self, name: String, lights: Vec<usize>, group_type: GroupType, room_class: Option<RoomClass>) -> HueFuture<usize> {
        let g = Group {
            name: name,
            lights: lights,
            group_type: group_type,
            class: room_class,
            state: None,
            action: None,
        };
        let uri = hyper::Uri::from_str(&format!("{}groups", self.url)).unwrap();
        let mut req = hyper::Request::new(hyper::Post, uri);
        let body = to_vec(&g).map_err(From::from);
        let future = futures::done(body).and_then(move |body| {
            req.set_body(body);
            self.send(req)
        }).and_then(|r: HueResponse<Id<usize>>|
                    future::done(r.into_result().map(|g| g.id))
        );
        Box::new(future)
    }
    /// Gets extra information about a specific group
    pub fn get_group_attributes(&self, id: usize) -> HueFuture<Group> {
        let uri = hyper::Uri::from_str(&format!("{}groups/{}", self.url, id)).unwrap();
        let req = hyper::Request::new(hyper::Get, uri);
        self.send(req)
    }
    /// Set the name, light and class of a group
    pub fn set_group_attributes(&self, id: usize, attr: &GroupCommand) -> HueFuture<SuccessVec> {
        let uri = hyper::Uri::from_str(&format!("{}groups/{}", self.url, id)).unwrap();
        let mut req = hyper::Request::new(hyper::Put, uri);
        let body = to_vec(attr).map_err(From::from);
        let future = futures::done(body).and_then(move |body| {
            req.set_body(body);
            self.send_and_extract(req)
        });
        Box::new(future)
    }
    /// Sets the state of all lights in the group.
    ///
    /// ID 0 is a sepcial group containing all lights known to the bridge
    pub fn set_group_state(&self, id: usize, state: &LightCommand) -> HueFuture<SuccessVec> {
        let uri = hyper::Uri::from_str(&format!("{}groups/{}/action", self.url, id)).unwrap();
        let mut req = hyper::Request::new(hyper::Put, uri);
        let body = to_vec(state).map_err(From::from);
        let future = futures::done(body).and_then(move |body| {
            req.set_body(body);
            self.send_and_extract(req)
        });
        Box::new(future)
    }
    /// Deletes the specified group
    ///
    /// It's not allowed to delete groups of type `LightSource` or `Luminaire`.
    pub fn delete_group(&self, id: usize) -> HueFuture<Vec<String>> {
        let uri = hyper::Uri::from_str(&format!("{}groups/{}", self.url, id)).unwrap();
        let req = hyper::Request::new(hyper::Delete, uri);
        self.send(req)
    }

    // CONFIGURATION

    /// Returns detailed information about the configuration of the bridge.
    pub fn get_configuration(&self) -> HueFuture<Configuration> {
        let uri = hyper::Uri::from_str(&format!("{}config", self.url)).unwrap();
        let req = hyper::Request::new(hyper::Get, uri);
        self.send(req)
    }
    /// Sets some configuration values.
    pub fn modify_configuration(&self, command: &ConfigurationModifier) -> HueFuture<SuccessVec> {
        let uri = hyper::Uri::from_str(&format!("{}config", self.url)).unwrap();
        let mut req = hyper::Request::new(hyper::Put, uri);
        let body = to_vec(command).map_err(From::from);
        let future = futures::done(body).and_then(move |body| {
            req.set_body(body);
            self.send_and_extract(req)
        });
        Box::new(future)
    }
    /// Deletes the specified user removing them from the whitelist.
    pub fn delete_user(&self, username: &str) -> HueFuture<Vec<String>> {
        let uri = hyper::Uri::from_str(&format!("{}config/whitelist/{}", self.url, username)).unwrap();
        let req = hyper::Request::new(hyper::Delete, uri);
        self.send_and_extract(req)
    }
    /// Fetches the entire datastore from the bridge.
    ///
    /// This is a resource intensive command for the bridge, and should therefore be used sparingly.
    pub fn get_full_state(&self) -> HueFuture<FullState> {
        let uri = hyper::Uri::from_str(&self.url).unwrap();
        let req = hyper::Request::new(hyper::Get, uri);
        self.send(req)
    }

    /// Sets the state of lights in the group to the state in the scene
    ///
    /// Note that this will affect that are both in the group and in the scene.
    /// Using group 0 will set all the lights in the scene, since group 0 is a special
    /// group that contains all lights
    pub fn recall_scene_in_group(&self, group_id: usize, scene_id: &str) -> HueFuture<SuccessVec> {
        let uri = hyper::Uri::from_str(&format!("{}groups/{}/action", self.url, group_id)).unwrap();
        let mut req = hyper::Request::new(hyper::Put, uri);
        let body = to_vec(&SceneRecall{scene: scene_id}).map_err(From::from);
        let future = futures::done(body).and_then(move |body| {
            req.set_body(body);
            self.send_and_extract(req)
        });
        Box::new(future)
    }

    // SCENES

    /// Gets all scenes of the bridge
    pub fn get_all_scenes(&self) -> HueFuture<BTreeMap<String, Scene>> {
        let uri = hyper::Uri::from_str(&format!("{}scenes", self.url)).unwrap();
        let req = hyper::Request::new(hyper::Get, uri);
        self.send(req)
    }
    /// Creates a scene on the bridge and returns the ID of the created scene.
    pub fn create_scene(&self, scene: &SceneCreater) -> HueFuture<String> {
        let uri = hyper::Uri::from_str(&format!("{}scenes", self.url)).unwrap();
        let mut req = hyper::Request::new(hyper::Post, uri);
        let body = to_vec(scene).map_err(From::from);
        let future = futures::done(body).and_then(move |body| {
            req.set_body(body);
            self.send(req)
        }).and_then(|r: HueResponse<Id<String>>|
                    futures::done(r.into_result().map(|g| g.id))
        );
        Box::new(future)
    }
    /// Sets general things in the specified scene
    pub fn modify_scene(&self, id: &str, scene: &SceneModifier) -> HueFuture<SuccessVec> {
        let uri = hyper::Uri::from_str(&format!("{}scenes/{}", self.url, id)).unwrap();
        let mut req = hyper::Request::new(hyper::Put, uri);
        let body = to_vec(scene).map_err(From::from);
        let future = futures::done(body).and_then(move |body| {
            req.set_body(body);
            self.send_and_extract(req)
        });
        Box::new(future)
    }
    /// Sets the light state of the specified ID that is stored in the scene
    pub fn set_light_state_in_scene(&self, scene_id: &str, light_id: usize,
        state: &LightStateChange) -> HueFuture<SuccessVec> {
        let uri = hyper::Uri::from_str(&format!("{}scenes/{}/lightstates/{}", self.url, scene_id, light_id)).unwrap();
        let mut req = hyper::Request::new(hyper::Put, uri);
        let body = to_vec(state).map_err(From::from);
        let future = futures::done(body).and_then(move |body| {
            req.set_body(body);
            self.send_and_extract(req)
        });
        Box::new(future)
    }
    /// Deletes the specified scene
    pub fn delete_scene(&self, id: &str) -> HueFuture<Vec<String>> {
        let uri = hyper::Uri::from_str(&format!("{}scenes/{}", self.url, id)).unwrap();
        let req = hyper::Request::new(hyper::Delete, uri);
        self.send_and_extract(req)
    }
    /// Gets the scene with the specified ID with its `lightstates`
    pub fn get_scene_with_states(&self, id: &str) -> HueFuture<Scene> {
        let uri = hyper::Uri::from_str(&format!("{}scenes/{}", self.url, id)).unwrap();
        let req = hyper::Request::new(hyper::Get, uri);
        self.send(req)
    }
}
