<?php
/** @psalm-suppress UndefinedClass */
$a = new A();
/**
 * @psalm-suppress UndefinedClass
 */
if (!isset($a->arr["bat"]) || strlen($a->arr["bat"])) { }
