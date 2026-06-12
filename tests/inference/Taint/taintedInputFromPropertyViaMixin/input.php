<?php
class A {
    public string $userId;

    public function __construct() {
        $this->userId = (string) $_GET["user_id"];
    }
}

/** @mixin A */
class B {
    private A $a;

    public function __construct(A $a) {
        $this->a = $a;
    }

    public function __get(string $name) {
        return $this->a->$name;
    }
}

class C {
    private B $b;

    public function __construct(B $b) {
        $this->b = $b;
    }

    public function getAppendedUserId() : string {
        return "aaaa" . $this->b->userId;
    }

    public function doDelete(PDO $pdo) : void {
        $userId = $this->getAppendedUserId();
        $this->deleteUser($pdo, $userId);
    }

    public function deleteUser(PDO $pdo, string $userId) : void {
        $pdo->exec("delete from users where user_id = " . $userId);
    }
}
