<?php
/**
 * @param mixed $width
 * @param mixed $height
 *
 * @throws RuntimeException
 */
function Foo($width, $height) : void {
    if (!is_numeric($width) || !is_numeric($height)) {
        throw new RuntimeException("Width & Height were not numeric!");
    }

    echo sprintf("padding-top:%s%%;", 100 * ($height/$width));
}