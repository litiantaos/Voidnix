use crate::infra::pinyin;

#[tauri::command]
pub fn match_keywords(query: String, keyword_sets: Vec<Vec<String>>) -> Vec<u32> {
    if query.trim().is_empty() {
        return vec![0; keyword_sets.len()];
    }

    pinyin::MATCHER.with(|matcher_cell| {
        let mut matcher = matcher_cell.borrow_mut();
        let pattern = pinyin::match_query(&query);
        let mut buf = Vec::new();

        keyword_sets
            .into_iter()
            .map(|keywords| {
                let combined = keywords.join(" ");
                pinyin::pinyin_score(&query, &combined, &pattern, &mut matcher, &mut buf)
            })
            .collect()
    })
}
