<?php

/** @var string $secret */
$length = strlen($secret);
if ($length > 16) {
    throw new Exception("");
}

assert($length === 1);
                    
