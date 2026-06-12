<?php
class StringUtility {
    /**
     * @psalm-assert-if-false !null $yStr
     */
    public static function isNull(?string $yStr): bool {
        if ($yStr === null) {
            return true;
        }
        return false;
    }
}

function test(?string $in) : void {
    $str = "test";
    if(!StringUtility::isNull($in)) {
        $str .= $in;
    }
}
