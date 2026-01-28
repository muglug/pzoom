<?php
interface One {}
interface Two {}


/**
 * @param One|Two $impl
 */
function a($impl) : void {
    if ($impl instanceof One && $impl instanceof Two) {
        throw new \Exception();
    } elseif ($impl instanceof One) {}
}

/**
 * @param One|Two $impl
 */
function b($impl) : void {
    if ($impl instanceof One && $impl instanceof Two) {
        throw new \Exception();
    } else {
        if ($impl instanceof One) {}
    }
}