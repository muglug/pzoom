<?php
function getCallable(): callable {
    return [DateTimeImmutable::class, "createFromFormat"];
}

$callable = getCallable();

if (!is_array($callable)) {
  exit;
}

[$classOrObject, $method] = $callable;
