<?php

/** @var array{invoice?: string, utd?: "utd", cancel_agreement?: "test", installment?: "test"} */
$b = [];


$buttons = [];
foreach ($b as $text) {
    $buttons[] = $text;
}
if (count($buttons) === 0) {
    echo "Zero";
}


/** @var ?string */
$test = null;
$urls = array_filter([$test]);

$mainUrlSet = false;
foreach ($urls as $_) {
    if (!$mainUrlSet) {
        $mainUrlSet = true;
    }
}
if (!$mainUrlSet) {
    echo "SKIP";
}


/**
 * @param string|list<bool|array{0:string, 1:string}> $time
 */
function mapTime($time): void
{
    $atime = is_array($time) ? $time : [];
    if ($time === "24h") {
        return;
    }

    for ($day = 0; $day < 7; ++$day) {
        if (!array_key_exists($day, $atime) || !is_array($atime[$day])) {
            continue;
        }

        $dayWh = $atime[$day];
        array_pop($dayWh);
    }
}
