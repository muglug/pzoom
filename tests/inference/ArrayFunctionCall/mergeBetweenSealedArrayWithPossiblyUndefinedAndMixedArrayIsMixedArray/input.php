<?php

function findit(Closure $x): void
{
    $closure = new ReflectionFunction($x);

    $statics = [];

    if (rand(0, 1)) {
        $statics = ["this" => "a"];
    }
    $b = $statics + $closure->getStaticVariables();
    /** @psalm-check-type $b = array<array-key, mixed> */

    $_a = count($b);

    /** @psalm-check-type $_a = int<0, max> */
}
                    
