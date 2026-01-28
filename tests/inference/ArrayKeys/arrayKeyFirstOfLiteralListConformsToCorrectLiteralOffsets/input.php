<?php
/**
 * @return 0|1
 */
function getKey() {
    return array_key_first(["foo", "bar"]);
}
