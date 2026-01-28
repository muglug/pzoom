<?php
class A {
    function __toString() {
        return "hello";
    }
}
echo (new A);
