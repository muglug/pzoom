<?php
class A {
    public function deleteUser(PDO $pdo) : void {
        $userId = (string) $_GET["user_id"];
        $pdo->exec("delete from users where user_id = " . $userId);
    }
}
