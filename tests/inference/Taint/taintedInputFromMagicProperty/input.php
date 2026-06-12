<?php
/**
 * @property string $userId
 */
class A {
    /** @var array<string, string> */
    private $vars = [];

    public function __get(string $s) : string {
        return $this->vars[$s];
    }

    public function __set(string $s, string $t) {
        $this->vars[$s] = $t;
    }
}

function getAppendedUserId() : void {
    $a = new A();
    $a->userId = (string) $_GET["user_id"];
    echo $a->userId;
}
