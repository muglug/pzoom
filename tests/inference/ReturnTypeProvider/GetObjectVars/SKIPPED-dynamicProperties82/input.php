<?php
#[AllowDynamicProperties]
class a {
    public function __construct(public string $t) {}
}

$a = new a("test");
$a->b = "test";
$test = get_object_vars($a);
