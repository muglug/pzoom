<?php
/**
 * @param mixed $width
 * @param mixed $height
 *
 * @throws RuntimeException
 */
function Foo($width, $height) : void {
    if (!(is_int($width) || is_float($width)) || !(is_int($height) || is_float($height))) {
        throw new RuntimeException("bad");
    }

    echo sprintf("padding-top:%s%%;", 100 * ($height/$width));
}