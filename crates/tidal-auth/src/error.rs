use thiserror::Error;

#[derive(Debug, Error)]
pub enum AuthError {
    #[error("identifiants client manquants : {0}")]
    MissingCredentials(&'static str),

    #[error("fichier d'identifiants illisible ({path}) : {source}")]
    CredentialsFile {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("requête HTTP : {0}")]
    Http(#[from] reqwest::Error),

    #[error("JSON : {0}")]
    Json(#[from] serde_json::Error),

    #[error("TIDAL a répondu HTTP {status} : {body}")]
    Api { status: u16, body: String },

    #[error("l'URL de redirection ne contient pas de paramètre 'code'")]
    MissingCode,

    #[error("le lien de connexion a expiré, recommencez")]
    DeviceExpired,

    #[error("connexion refusée par l'utilisateur")]
    AccessDenied,

    #[error("aucun token enregistré : lancez d'abord `login`")]
    NoToken,

    #[error("aucun refresh_token disponible")]
    NoRefreshToken,

    #[error("Gestionnaire d'identification Windows : {0}")]
    Store(String),

    #[error("URL invalide : {0}")]
    Url(#[from] url::ParseError),

    #[error("base64 : {0}")]
    Base64(#[from] base64::DecodeError),

    #[error("générateur aléatoire système : {0}")]
    Random(String),

    #[error("manifest : {0}")]
    Manifest(String),
}

pub type Result<T> = std::result::Result<T, AuthError>;

impl AuthError {
    /// Vrai si l'erreur correspond à un access_token expiré/invalide.
    pub fn is_unauthorized(&self) -> bool {
        matches!(self, AuthError::Api { status: 401, .. })
    }
}
