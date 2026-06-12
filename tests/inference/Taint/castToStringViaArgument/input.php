<?php
class MyClass {
    public function __toString() {
        return $_GET["blah"];
    }
}

function doesEcho(string $s): void {
    echo $s;
}

$unsafe = new MyClass();

doesEcho($unsafe);
