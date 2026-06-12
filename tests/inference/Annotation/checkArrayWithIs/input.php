<?php
/** @param mixed $b */
function foo($b): void {
    /**
     * @psalm-suppress UnnecessaryVarAnnotation
     * @var array
     */
    $a = (array)$b;
    if (is_array($a)) {
        // do something
    }
}
