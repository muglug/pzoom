<?php
class O {
    public function foo() : bool { return true; }
}

/** @var mixed */
$value = $_GET["foo"];

$a = is_string($value) && (($value = rand(0, 1) ? new O : null) !== null) && $value->foo();