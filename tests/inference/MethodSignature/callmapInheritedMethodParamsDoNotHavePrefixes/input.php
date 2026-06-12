<?php

class NoopFilter extends \php_user_filter
{
    /**
     * @param resource $in
     * @param resource $out
     * @param int $consumed   -- this is called &rw_consumed in the callmap
     */
    public function filter($in, $out, &$consumed, bool $closing): int
    {
        return PSFS_PASS_ON;
    }
}
