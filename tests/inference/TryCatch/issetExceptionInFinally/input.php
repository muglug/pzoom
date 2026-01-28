<?php
try {
    if (rand(0, 1)) {
        throw new \Exception("bad");
    }
} catch (Throwable $exception) {
    //throw $exception;
} finally {
    if (isset($exception)) {}
}
