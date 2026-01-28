<?php
class ParentClass {
    public static function __callStatic(string $name, array $args) {}
}

/**
 * @method static string getString(int $foo) with some more text
 */
class Child extends ParentClass {}

$child = new Child();

$a = $child::getString(5);
