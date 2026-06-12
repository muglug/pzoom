<?php
/**
 * @psalm-assert-untainted $userId
 */
function validateUserId(string $userId) : void {
    if (!is_numeric($userId)) {
        throw new \Exception("bad");
    }
}

class A {
    public function getUserId() : string {
        return (string) $_GET["user_id"];
    }

    public function doDelete(PDO $pdo) : void {
        $userId = $this->getUserId();
        validateUserId($userId);
        $this->deleteUser($pdo, $userId);
    }

    public function deleteUser(PDO $pdo, string $userId) : void {
        $pdo->exec("delete from users where user_id = " . $userId);
    }
}
