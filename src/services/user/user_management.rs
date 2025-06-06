use argon2::{
    password_hash::{rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use uuid::Uuid;
use validator::Validate;

use crate::db::error::DatabaseError;
use crate::db::repositories::UserRepository;
use crate::errors::AppError;
use crate::models::user::{CreateUserDto, UpdateUserDto, User, UserResponse};
use crate::services::validation::validation_err_to_app_error;

pub struct UserManagementService {
    user_repo: UserRepository,
}

impl UserManagementService {
    pub fn new(user_repo: UserRepository) -> Self {
        Self { user_repo }
    }

    // Register new user
    pub async fn register_user(&self, dto: CreateUserDto) -> Result<User, AppError> {
        // Validate DTO
        dto.validate().map_err(validation_err_to_app_error)?;

        // Hash password using Argon2
        let password_hash = self.hash_password(&dto.password)?;

        // Save user to database
        let user = self
            .user_repo
            .create(&dto, password_hash)
            .await
            .map_err(AppError::Database)?;

        Ok(user)
    }

    // Get user data by ID
    pub async fn get_user_by_id(&self, id: Uuid) -> Result<UserResponse, AppError> {
        let user = self.user_repo.find_by_id(id).await.map_err(|e| match e {
            DatabaseError::NotFound => AppError::NotFound("User not found".into()),
            _ => AppError::Database(e),
        })?;

        Ok(UserResponse::from(user))
    }

    // Get user data by email
    pub async fn get_user_by_email(&self, email: &str) -> Result<UserResponse, AppError> {
        let user = self
            .user_repo
            .find_by_email(email)
            .await
            .map_err(|e| match e {
                DatabaseError::NotFound => AppError::NotFound("User not found".into()),
                _ => AppError::Database(e),
            })?;

        Ok(UserResponse::from(user))
    }

    // Get all users with pagination
    pub async fn get_all_users(
        &self,
        page: i64,
        limit: i64,
    ) -> Result<(Vec<UserResponse>, u64), AppError> {
        // Calculate offset from page
        let offset = (page - 1) * limit;

        // Get users
        let users = self
            .user_repo
            .find_all(limit, offset)
            .await
            .map_err(AppError::Database)?;

        // Get total count
        let total = self.user_repo.count().await.map_err(AppError::Database)? as u64;

        // Convert to UserResponse
        let user_responses = users.into_iter().map(UserResponse::from).collect();

        Ok((user_responses, total))
    }

    // Update user data
    pub async fn update_user(
        &self,
        id: Uuid,
        dto: UpdateUserDto,
    ) -> Result<UserResponse, AppError> {
        // Validate DTO
        dto.validate().map_err(validation_err_to_app_error)?;

        // Update user in database
        let user = self.user_repo.update(id, &dto).await.map_err(|e| match e {
            DatabaseError::NotFound => AppError::NotFound("User not found".into()),
            DatabaseError::Duplicate(msg) => AppError::Validation(msg),
            _ => AppError::Database(e),
        })?;

        Ok(UserResponse::from(user))
    }

    // Update user password
    pub async fn update_password(
        &self,
        id: Uuid,
        current_password: &str,
        new_password: &str,
    ) -> Result<(), AppError> {
        // Validate new password
        crate::services::validation::validate_password_strength(new_password)
            .map_err(|e| AppError::Validation(e.to_string()))?;

        // Get user data
        let user = self.user_repo.find_by_id(id).await.map_err(|e| match e {
            DatabaseError::NotFound => AppError::NotFound("User not found".into()),
            _ => AppError::Database(e),
        })?;

        // Verify current password
        self.verify_password(current_password, &user.password_hash)?;

        // Hash new password
        let new_password_hash = self.hash_password(new_password)?;

        // Update password in database
        self.user_repo
            .update_password(id, &new_password_hash)
            .await
            .map_err(AppError::Database)?;

        Ok(())
    }

    // Update user password directly (for admin)
    pub async fn update_user_password(&self, id: Uuid, new_password: &str) -> Result<(), AppError> {
        // Validate new password
        crate::services::validation::validate_password_strength(new_password)
            .map_err(|e| AppError::Validation(e.to_string()))?;

        // Hash new password
        let new_password_hash = self.hash_password(new_password)?;

        // Update password in database
        self.user_repo
            .update_password(id, &new_password_hash)
            .await
            .map_err(|e| match e {
                DatabaseError::NotFound => AppError::NotFound("User not found".into()),
                _ => AppError::Database(e),
            })?;

        Ok(())
    }

    // Delete user (soft delete)
    pub async fn delete_user(&self, id: Uuid) -> Result<(), AppError> {
        self.user_repo.delete(id).await.map_err(|e| match e {
            DatabaseError::NotFound => AppError::NotFound("User not found".into()),
            _ => AppError::Database(e),
        })?;

        Ok(())
    }

    // Verify user email
    pub async fn verify_email(&self, id: Uuid) -> Result<UserResponse, AppError> {
        let user = self
            .user_repo
            .update_email_verification(id, true)
            .await
            .map_err(|e| match e {
                DatabaseError::NotFound => AppError::NotFound("User not found".into()),
                _ => AppError::Database(e),
            })?;

        Ok(UserResponse::from(user))
    }

    // Helper function to hash password
    pub fn hash_password(&self, password: &str) -> Result<String, AppError> {
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();

        argon2
            .hash_password(password.as_bytes(), &salt)
            .map(|hash| hash.to_string())
            .map_err(|e| AppError::Internal(format!("Failed to hash password: {}", e)))
    }

    // Helper function to verify password
    pub fn verify_password(&self, password: &str, hash: &str) -> Result<(), AppError> {
        let parsed_hash = PasswordHash::new(hash)
            .map_err(|e| AppError::Internal(format!("Invalid password hash format: {}", e)))?;

        Argon2::default()
            .verify_password(password.as_bytes(), &parsed_hash)
            .map_err(|_| AppError::Authentication("Email or password incorrect".into()))
    }
}
