<?php
function bar(bool $result): bool {
    if ($result && ($result = rand(0, 1))) {
        return true;
    } else {
        return $result;
    }
}
