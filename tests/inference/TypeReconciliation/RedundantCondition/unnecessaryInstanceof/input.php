<?php
class One {
    public function fooFoo() : void {}
}

$var = new One();

if ($var instanceof One) {
    $var->fooFoo();
}
