<?php
/**
 * @return int|false
 */
function badpattern() {
    return @preg_match_all("foo", "foo", $matches);
}
