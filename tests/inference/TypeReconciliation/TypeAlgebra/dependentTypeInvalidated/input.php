<?php
class A {
    public function isValid() : bool {
        return (bool) rand(0, 1);
    }

    public function foo() : void {}
}

function takesA(?A $a) : void {
    $is_valid_a = $a && $a->isValid();

    if (rand(0, 1)) {
        $is_valid_a = false;
    }

    if ($is_valid_a) {
        $a->foo();
    }
}
