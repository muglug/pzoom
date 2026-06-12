<?php
class A {
    public function getUserId(PDO $pdo) : void {
        $this->deleteUser(
            $pdo,
            self::doFoo(),
            $this->getAppendedUserId((string) $_GET["user_id"])
        );
    }

    public function getAppendedUserId(string $user_id) : string {
        return "aaa" . $user_id;
    }

    public static function doFoo() : string {
        return "hello";
    }

    public function deleteUser(PDO $pdo, string $userId, string $userId2) : void {
        $pdo->exec("delete from users where user_id = " . $userId);

        if (rand(0, 1)) {
            $pdo->exec("delete from users where user_id = " . $userId2);
        }
    }
}
