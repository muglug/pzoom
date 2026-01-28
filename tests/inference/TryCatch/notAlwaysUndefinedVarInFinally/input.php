<?php
function maybeThrows() : void {
    if (rand(0, 1)) {
        throw new UnexpectedValueException();
    }
}

function doTry() : void {
    try {
        maybeThrows();
        return;
    } catch (Exception $exception) {
        throw $exception;
    } finally {
        if (isset($exception)) {
            echo "here";
        }
    }
}
