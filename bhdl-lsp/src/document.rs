//! Document store for managing open BHDL files

use std::collections::HashMap;
use tower_lsp::lsp_types::Url;

/// Stores open documents
pub struct DocumentStore {
    documents: HashMap<Url, Document>,
}

pub struct Document {
    pub text: String,
}

impl DocumentStore {
    pub fn new() -> Self {
        Self {
            documents: HashMap::new(),
        }
    }

    pub fn open(&mut self, uri: Url, text: String) {
        self.documents.insert(uri, Document {
            text,
        });
    }

    pub fn update(&mut self, uri: Url, text: String) {
        if let Some(doc) = self.documents.get_mut(&uri) {
            doc.text = text;
        }
    }

    pub fn close(&mut self, uri: Url) {
        self.documents.remove(&uri);
    }

    pub fn get(&self, uri: &Url) -> Option<&Document> {
        self.documents.get(uri)
    }
}
