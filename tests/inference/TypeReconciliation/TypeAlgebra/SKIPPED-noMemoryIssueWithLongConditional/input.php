<?php

function foo(int $c) : string {
    if (!($c >= 0x5be && $c <= 0x10b7f)) {
        return "LTR";
    }

    if ($c <= 0x85e) {
        if ($c === 0x5be ||
            $c === 0x5c0 ||
            $c === 0x5c3 ||
            $c === 0x5c6 ||
            ($c >= 0x5d0 && $c <= 0x5ea) ||
            ($c >= 0x5f0 && $c <= 0x5f4) ||
            $c === 0x608 ||
            ($c >= 0x712 && $c <= 0x72f) ||
            ($c >= 0x74d && $c <= 0x7a5) ||
            $c === 0x7b1 ||
            ($c >= 0x7c0 && $c <= 0x7ea) ||
            ($c >= 0x7f4 && $c <= 0x7f5) ||
            $c === 0x7fa ||
            ($c >= 0x800 && $c <= 0x815) ||
            $c === 0x81a ||
            $c === 0x824 ||
            $c === 0x828 ||
            ($c >= 0x830 && $c <= 0x83e) ||
            ($c >= 0x840 && $c <= 0x858) ||
            $c === 0x85e
        ) {
            return "RTL";
        }
    } elseif ($c === 0x200f) {
        return "RTL";
    } elseif ($c >= 0xfb1d) {
        if ($c === 0xfb1d ||
            ($c >= 0xfb1f && $c <= 0xfb28) ||
            ($c >= 0xfb2a && $c <= 0xfb36) ||
            ($c >= 0xfb38 && $c <= 0xfb3c) ||
            $c === 0xfb3e ||
            ($c >= 0x10a10 && $c <= 0x10a13) ||
            ($c >= 0x10a15 && $c <= 0x10a17) ||
            ($c >= 0x10a19 && $c <= 0x10a33) ||
            ($c >= 0x10a40 && $c <= 0x10a47) ||
            ($c >= 0x10a50 && $c <= 0x10a58) ||
            ($c >= 0x10a60 && $c <= 0x10a7f) ||
            ($c >= 0x10b00 && $c <= 0x10b35) ||
            ($c >= 0x10b40 && $c <= 0x10b55) ||
            ($c >= 0x10b58 && $c <= 0x10b72) ||
            ($c >= 0x10b78 && $c <= 0x10b7f)
        ) {
            return "RTL";
        }
    }

    return "LTR";
}