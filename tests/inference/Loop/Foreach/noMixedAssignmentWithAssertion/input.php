<?php
$object = new stdClass();
$reflection = new ReflectionClass($object);

foreach ($reflection->getProperties() as $property) {
    $message = $property->getValue($reflection->newInstance());
    assert(is_string($message));
}
