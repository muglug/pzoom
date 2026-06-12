<?php
$method = new ReflectionMethod(stdClass::class);
$parameters = $method->getParameters();
foreach ($parameters as $parameter) {
    if ($parameter->hasType()) {
        $parameter->getType()->__toString();
    }
}
