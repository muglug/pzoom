<?php
$class = new ReflectionClass(stdClass::class);
$properties = $class->getProperties();
foreach ($properties as $property) {
    if ($property->hasType()) {
        $property->getType()->allowsNull();
    }
}
