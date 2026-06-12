<?php
class A {
    public function getUserId() : string {
        return (string) $_GET["user_id"];
    }

    public function getAppendedUserId() : string {
        return "aaaa" . $this->getUserId();
    }

    public function doDelete(PDO $pdo) : void {
        $userId = $this->getAppendedUserId();
        $this->deleteUser($pdo, $userId);
    }

    public function deleteUser(PDO $pdo, string $userId) : void {
        $pdo->exec("delete from users where user_id = " . $userId);
    }
}
