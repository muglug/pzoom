<?php
class C {
    /** @var string */
    public $name = "";
    public function foo(): void {}
}

foreach (array_column([["name" => "", "instance" => new C]], null, "name") as $array) {
    $array["instance"]->foo();
}
            
