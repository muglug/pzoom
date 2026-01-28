<?php
/** @return void */
function foo(Exception $e) {
    switch (get_class($e)) {
        case InvalidArgumentException::class:
            $e->getMessage();
            break;

        case LogicException::class:
            $e->getMessage();
            break;
    }
}
