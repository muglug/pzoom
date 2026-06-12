<?php
class A {
    public string $userId;

    public function __construct() {
        $this->userId = (string) $_GET["user_id"];
    }

    public function getAppendedUserId() : string {
        return "aaaa" . $this->userId;
    }

    public function doDelete(PDO $pdo) : void {
        $userId = $this->getAppendedUserId();
        $this->deleteUser($pdo, $userId);
    }

    public function deleteUser(PDO $pdo, string $userId) : void {
        $pdo->exec("delete from users where user_id = " . $userId);
    }
}
