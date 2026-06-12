<?php
class A {
    public function getUserId() : string {
        return (string) $_GET["user_id"];
    }

    public function getAppendedUserId() : string {
        $userId = $this->getUserId();

        if (rand(0, 1)) {
            $userId .= "aaa";
        } else {
            $userId .= "bb";
        }

        return $userId;
    }

    public function deleteUser(PDO $pdo) : void {
        $userId = $this->getAppendedUserId();
        $pdo->exec("delete from users where user_id = " . $userId);
    }
}
