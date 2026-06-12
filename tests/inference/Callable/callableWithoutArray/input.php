<?php
/**
 * @param array|(callable():array) $var
 */
function text($var): array
{
    if (is_array($var)) {
        return $var;
    }

    //callable-string can't specify return type but it doesn't error
    return call_user_func($var);
}
