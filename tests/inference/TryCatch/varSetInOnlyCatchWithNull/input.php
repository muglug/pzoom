<?php
$lastException = null;

try {
    if (rand(0, 1)) {
        throw new \Exception("Gotcha!");
    }

    exit;
} catch (\Exception $e) {
    $lastException = $e;
}

echo $lastException->getMessage();
