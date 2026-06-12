<?php
class A {
    /** @var array<string, string> */
    public $arr = [];
}
$a = new A();
if (!isset($a->arr["bat"]) || strlen($a->arr["bat"])) { }
