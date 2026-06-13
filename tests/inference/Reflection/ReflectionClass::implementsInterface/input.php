<?php
$a = new ReflectionClass(stdClass::class);
if (!$a->implementsInterface(Iterator::class)) {
    throw new Exception();
}
