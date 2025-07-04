use std::sync::Arc;

use axum::{
    extract::{Extension, Json, Path, Query, State},
    http::StatusCode,
    response::Response,
};
use uuid::Uuid;

use crate::config::AppConfig;
use crate::db::repositories::Repositories;
use crate::errors::AppError;
use crate::middleware::auth::Claims;
use crate::models::common::pagination::PaginationQuery;
use crate::models::common::response::{ApiResponse, PaginatedResponse};
use crate::models::user::{
    CreateUserDto, UpdatePasswordDto, UpdateUserDto, UserResponse, GLOBAL_ROLE_ADMIN,
};
use crate::services::auth::AuthService;
use crate::services::user::UserManagementService;

// Get all users with pagination
pub async fn list_users(
    Extension(_claims): Extension<Claims>,
    Query(pagination): Query<PaginationQuery>,
    State((_repos, _, user_management, _auth_service)): State<(
        Arc<Repositories>,
        AppConfig,
        Arc<UserManagementService>,
        Arc<AuthService>,
    )>,
) -> Result<Response, AppError> {
    // Admin check is now handled by middleware
    let (users, total) = user_management
        .get_all_users(pagination.page, pagination.limit)
        .await?;

    let total_pages = (total as f64 / pagination.limit as f64).ceil() as i64;

    let response = PaginatedResponse {
        data: users,
        total: total as i64,
        page: pagination.page,
        limit: pagination.limit,
        total_pages,
    };

    Ok(ApiResponse::success(StatusCode::OK, response))
}

// Get current user
pub async fn get_current_user(
    Extension(_claims): Extension<Claims>,
    State((_, _, user_management, _auth_service)): State<(
        Arc<Repositories>,
        AppConfig,
        Arc<UserManagementService>,
        Arc<AuthService>,
    )>,
) -> Result<Response, AppError> {
    let user_id = Uuid::parse_str(&_claims.sub).unwrap();
    let user = user_management.get_user_by_id(user_id).await?;
    Ok(ApiResponse::success(StatusCode::OK, user))
}

// Get user by ID
pub async fn get_user(
    Path(id): Path<Uuid>,
    State((_, _, user_management, _auth_service)): State<(
        Arc<Repositories>,
        AppConfig,
        Arc<UserManagementService>,
        Arc<AuthService>,
    )>,
) -> Result<Response, AppError> {
    let user = user_management.get_user_by_id(id).await?;
    Ok(ApiResponse::success(StatusCode::OK, user))
}

// Create a new user (admin only)
pub async fn create_user(
    Extension(_claims): Extension<Claims>,
    State((_, _, user_management, _auth_service)): State<(
        Arc<Repositories>,
        AppConfig,
        Arc<UserManagementService>,
        Arc<AuthService>,
    )>,
    Json(create_dto): Json<CreateUserDto>,
) -> Result<Response, AppError> {
    // Admin check is now handled by middleware
    let user = user_management.register_user(create_dto).await?;
    let user_response = UserResponse::from(user);

    Ok(ApiResponse::created(user_response))
}

// Update current user
pub async fn update_current_user(
    Extension(_claims): Extension<Claims>,
    State((_, _, user_management, _auth_service)): State<(
        Arc<Repositories>,
        AppConfig,
        Arc<UserManagementService>,
        Arc<AuthService>,
    )>,
    Json(update_dto): Json<UpdateUserDto>,
) -> Result<Response, AppError> {
    let user_id = Uuid::parse_str(&_claims.sub).unwrap();

    // If not admin, they can't modify the is_active field
    if _claims.role != GLOBAL_ROLE_ADMIN && update_dto.is_active.is_some() {
        return Err(crate::errors::AppError::Authorization(
            "Access denied. Only administrators can change a user's active status.".into(),
        ));
    }

    let user = user_management.update_user(user_id, update_dto).await?;
    Ok(ApiResponse::success(StatusCode::OK, user))
}

// Update user
pub async fn update_user(
    Extension(_claims): Extension<Claims>,
    Path(id): Path<Uuid>,
    State((_, _, user_management, _auth_service)): State<(
        Arc<Repositories>,
        AppConfig,
        Arc<UserManagementService>,
        Arc<AuthService>,
    )>,
    Json(update_dto): Json<UpdateUserDto>,
) -> Result<Response, AppError> {
    // Users can only update their own data, unless they are admin
    if _claims.sub != id.to_string() && _claims.role != GLOBAL_ROLE_ADMIN {
        return Err(crate::errors::AppError::Authorization(
            "Access denied. You can only modify your own data.".into(),
        ));
    }

    // If not admin, they can't modify the is_active field
    if _claims.role != GLOBAL_ROLE_ADMIN && update_dto.is_active.is_some() {
        return Err(crate::errors::AppError::Authorization(
            "Access denied. Only administrators can change a user's active status.".into(),
        ));
    }

    let user = user_management.update_user(id, update_dto).await?;
    Ok(ApiResponse::success(StatusCode::OK, user))
}

// Delete user (soft delete)
pub async fn delete_user(
    Extension(_claims): Extension<Claims>,
    Path(id): Path<Uuid>,
    State((_, _, user_management, _auth_service)): State<(
        Arc<Repositories>,
        AppConfig,
        Arc<UserManagementService>,
        Arc<AuthService>,
    )>,
) -> Result<Response, AppError> {
    // Admin check is handled by middleware
    user_management.delete_user(id).await?;
    Ok(ApiResponse::no_content())
}

// Update current user's password
pub async fn update_current_user_password(
    Extension(_claims): Extension<Claims>,
    State((_, _, user_management, _auth_service)): State<(
        Arc<Repositories>,
        AppConfig,
        Arc<UserManagementService>,
        Arc<AuthService>,
    )>,
    Json(password_request): Json<UpdatePasswordDto>,
) -> Result<Response, AppError> {
    let user_id = Uuid::parse_str(&_claims.sub).unwrap();

    // Use the user management service to update the password
    user_management
        .update_password(
            user_id,
            &password_request.current_password,
            &password_request.new_password,
        )
        .await?;

    Ok(ApiResponse::success(
        StatusCode::OK,
        "Password updated successfully",
    ))
}

// Update any user's password (admin only)
pub async fn update_user_password(
    Extension(_claims): Extension<Claims>,
    Path(id): Path<Uuid>,
    State((_, _, user_management, _auth_service)): State<(
        Arc<Repositories>,
        AppConfig,
        Arc<UserManagementService>,
        Arc<AuthService>,
    )>,
    Json(password_request): Json<UpdatePasswordDto>,
) -> Result<Response, AppError> {
    // Only admin can change other users' passwords
    if _claims.sub != id.to_string() && _claims.role != GLOBAL_ROLE_ADMIN {
        return Err(crate::errors::AppError::Authorization(
            "Access denied. You can only change your own password.".into(),
        ));
    }

    // If it's admin changing another user's password, we don't need to verify the current password
    if _claims.sub != id.to_string() && _claims.role == GLOBAL_ROLE_ADMIN {
        user_management
            .update_user_password(id, &password_request.new_password)
            .await?;
    } else {
        // For users changing their own passwords, we need to verify with the update_password method
        user_management
            .update_password(
                id,
                &password_request.current_password,
                &password_request.new_password,
            )
            .await?;
    }

    Ok(ApiResponse::success(
        StatusCode::OK,
        "Password updated successfully",
    ))
}
