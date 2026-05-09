use std::path::Path;
use kazane_game_launcher::data::remote::GameList;

#[test]
fn game_list_search_partial_and_tag() {
    let path = Path::new("data/game_list.json");
    let gl: GameList = kazane_game_launcher::utils::file::read_json_with_lock(path).expect("load game_list");

    // partial match by name/id
    let res = gl.search("sample", None);
    assert!(res.len() >= 2, "expected at least 2 matches for 'sample', got {}", res.len());

    // partial id match
    let res2 = gl.search("exe", None);
    assert!(res2.iter().any(|g| g.id == "exe-sample"));

    // tag search for the sample-tag should return zip-sample only
    let res3 = gl.search("", Some(&["sample-tag"]));
    assert_eq!(res3.len(), 1);
    assert_eq!(res3[0].id, "zip-sample");
}
