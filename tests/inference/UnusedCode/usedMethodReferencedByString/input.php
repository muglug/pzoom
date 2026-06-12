<?php
final class A {
    static function b(): void {}
}
$methodRef = "A::b";
$methodRef();
