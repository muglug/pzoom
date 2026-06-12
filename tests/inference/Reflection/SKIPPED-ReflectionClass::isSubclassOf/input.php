<?php
$a = new ReflectionClass(stdClass::class);
if (!$a->isSubclassOf(Iterator::class)) {
    throw new Exception();
}
