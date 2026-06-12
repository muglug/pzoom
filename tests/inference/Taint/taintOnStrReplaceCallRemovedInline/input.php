<?php
class V {}

class O1 {
    public string $s;

    public function __construct() {
        $this->s = (string) $_GET["FOO"];
    }
}

class V1 extends V {
    public function foo(O1 $o) : void {
        /**
         * @psalm-taint-escape html
         * @psalm-taint-escape has_quotes
         */
        $a = str_replace("foo", "bar", $o->s);
        echo $a;
    }
}
