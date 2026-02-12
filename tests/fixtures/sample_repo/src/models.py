"""Data models for the application."""

from dataclasses import dataclass, field
from typing import List, Optional
from datetime import datetime


@dataclass
class User:
    """Represents a user in the system."""
    id: int
    username: str
    email: str
    created_at: datetime = field(default_factory=datetime.now)
    roles: List[str] = field(default_factory=list)

    def has_role(self, role: str) -> bool:
        return role in self.roles

    def add_role(self, role: str) -> None:
        if role not in self.roles:
            self.roles.append(role)


class UserRepository:
    """Manages persistence of User objects."""

    def __init__(self):
        self._users = {}

    def save(self, user: User) -> None:
        self._users[user.id] = user

    def find_by_id(self, user_id: int) -> Optional[User]:
        return self._users.get(user_id)

    def find_by_username(self, username: str) -> Optional[User]:
        for user in self._users.values():
            if user.username == username:
                return user
        return None

    def count(self) -> int:
        return len(self._users)


@dataclass
class Permission:
    """Represents a permission that can be assigned to roles."""
    name: str
    description: str
    resource: str
    action: str


def create_default_admin() -> User:
    """Create a default admin user for initial setup."""
    admin = User(id=1, username="admin", email="admin@example.com")
    admin.add_role("admin")
    return admin


def validate_email(email: str) -> bool:
    """Basic email validation."""
    return "@" in email and "." in email.split("@")[1]
