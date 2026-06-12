<?php
class A {
    public function getUserId(PDO $pdo) : void {
        $this->deleteUser(
            $pdo,
            $this->getAppendedUserId((string) $_GET["user_id"])
        );
    }

    public function getAppendedUserId(string $user_id) : string {
        return "aaa" . $user_id;
    }

    public function deleteUser(PDO $pdo, string $userId) : void {
        $pdo->exec("delete from users where user_id = " . $userId);
    }
}
