<?php
$object = new stdClass();
$reflection = new ReflectionClass($object);

foreach ($reflection->getProperties() as $property) {
    $message = $property->getValue($reflection->newInstance());

    if (!is_string($message)) {
        throw new RuntimeException();
    }
}
