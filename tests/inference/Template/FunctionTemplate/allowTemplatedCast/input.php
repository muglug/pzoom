<?php
/**
 * @psalm-template Tk of array-key
 * @psalm-param Tk $key
 */
function at($key) : void {
    echo (string) $key;
}
