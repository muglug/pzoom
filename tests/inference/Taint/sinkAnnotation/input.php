<?php
class A {
    public function getUserId() : string {
        return (string) $_GET["user_id"];
    }

    public function getAppendedUserId() : string {
        return "aaaa" . $this->getUserId();
    }

    public function deleteUser(PDOWrapper $pdo) : void {
        $userId = $this->getAppendedUserId();
        $pdo->exec("delete from users where user_id = " . $userId);
    }
}

class PDOWrapper {
    /**
     * @psalm-taint-sink sql $sql
     */
    public function exec(string $sql) : void {}
}
