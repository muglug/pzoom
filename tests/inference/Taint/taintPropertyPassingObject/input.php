<?php
/** @psalm-immutable */
class User {
    public string $id;

    public function __construct(string $userId) {
        $this->id = $userId;
    }
}

class UserUpdater {
    public static function doDelete(PDO $pdo, User $user) : void {
        self::deleteUser($pdo, $user->id);
    }

    public static function deleteUser(PDO $pdo, string $userId) : void {
        $pdo->exec("delete from users where user_id = " . $userId);
    }
}

$userObj = new User((string) $_GET["user_id"]);
UserUpdater::doDelete(new PDO("test"), $userObj);
