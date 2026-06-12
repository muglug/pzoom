<?php
class Arr {
    /**
     * @param mixed $c
     * @return mixed
     */
    public static function pull(array &$a, string $b, $c = null) {
        return $a[$b] ?? $c;
    }
}

function _renderButton(array $settings): void {
    Arr::pull($settings, "a", true);

    if (isset($settings["b"])) {
        Arr::pull($settings, "b");
    }

    if (isset($settings["c"])) {}
}
