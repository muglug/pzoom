<?php
class StringUtility {
    /**
     * @psalm-assert-if-true !null $yStr
     */
    public static function isNotNull(?string $yStr): bool {
        if ($yStr === null) {
            return true;
        }
        return false;
    }
}

function test(?string $in) : void {
    $str = "test";
    if(StringUtility::isNotNull($in)) {
        $str .= $in;
    }
}
