<?php
class A {
    public function deleteUser(PDOWrapper $pdo) : void {
        /**
         * @psalm-taint-escape sql
         */
        $userId = (string) $_GET["user_id"];
        $pdo->exec("delete from users where user_id = " . $userId);
    }

    public function deleteUserSafer(PDOWrapper $pdo) : void {
        $userId = $this->getSafeId();
        $pdo->exec("delete from users where user_id = " . $userId);
    }

    public function getSafeId() : string {
        return "5";
    }
}

class PDOWrapper {
    /**
     * @psalm-taint-sink sql $sql
     */
    public function exec(string $sql) : void {}
}
