<?php
/** @param array{f: mixed} $a */
function func(array &$a): void
{
    $_ = &$a["f"];
}
                
