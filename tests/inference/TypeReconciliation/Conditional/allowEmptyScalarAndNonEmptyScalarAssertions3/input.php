<?php
/** @param mixed $value */
function foo($value) : void {
    if (\is_scalar($value)) {
        if ($value) {
            if ($value) {}
        } else {
            echo $value;
        }
    }
}
