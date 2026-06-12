<?php
class Foo {
    public function __toString() {
        return "Foo";
    }
}

$a = ["Foo" => "bar"];
echo $a[new Foo];
