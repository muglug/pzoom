<?php
$a = new stdClass();
$b = new ReflectionClass(Iterator::class);
if (!$b->isInstance($a)) {
    throw new Exception();
}
