<?php
/** @return ReflectionClass<object> */
function takesString(string $s) {
    /** @psalm-suppress ArgumentTypeCoercion */
    return new ReflectionClass($s);
}
