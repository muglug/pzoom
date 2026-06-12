<?php
class MyClass {
    public function __toString() {
        return $_GET["blah"];
    }
}
$unsafe = new MyClass();
echo $unsafe;
