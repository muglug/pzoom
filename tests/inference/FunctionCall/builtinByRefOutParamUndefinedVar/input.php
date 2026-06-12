<?php
function f(int $pid, bool $wait): ?int {
    if (pcntl_waitpid($pid, $status, $wait ? 0 : WNOHANG) === 0) {
        return null;
    }
    /** @var int $status */
    return $status;
}
