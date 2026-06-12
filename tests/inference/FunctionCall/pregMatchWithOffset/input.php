<?php
/** @param string[] $matches */
function takesMatches(array $matches) : void {}

preg_match("{foo}", "foo", $matches, 0, 10);

takesMatches($matches);
