<?php
/** @param mixed $value */
function foo($value) : void {
    if (\is_scalar($value)) {
        if ($value) {
            echo $value;
        } else {
            if (\is_scalar($value)) {}
        }
    }
}
