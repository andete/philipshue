#[derive(Debug, Deserialize)]
/// A user object returned from the API
pub struct User{
    /// The username of the user
    pub username: String
}

#[derive(Debug, Deserialize)]
/// An object containing the ID of a newly created Group
pub struct GroupId{
    /// The ID of the group
    pub id: usize
}
