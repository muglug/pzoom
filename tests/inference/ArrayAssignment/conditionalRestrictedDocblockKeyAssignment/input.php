<?php


/**
 * @return array{booking: array{active: false, icon: "settings"}, phones: array{active: false, icon: "phone-tube"}, stat: array{active: false, icon: "review"}, support: array{active: false, icon: "help"}}
 */
function getSections(): array {
    return [
            "phones" => [
                "active" => false,
                "icon" => "phone-tube",
            ],
            "stat" => [
                "active" => false,
                "icon" => "review",
            ],
            "booking" => [
                "active" => false,
                "icon" => "settings",
            ],
            "support" => [
                "active" => false,
                "icon" => "help",
            ],
    ];
}
$items = getSections();
/** @var string */
$currentAction = "";
if (\array_key_exists($currentAction, $items)) {
    $items[$currentAction]["active"] = true;
}
