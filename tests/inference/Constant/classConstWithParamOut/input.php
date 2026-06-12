<?php

class Reconciler
{
    public const RECONCILIATION_OK = 0;
    public const RECONCILIATION_EMPTY = 1;

    public static function reconcileKeyedTypes(): void
    {

        $failed_reconciliation = 0;

        self::boo($failed_reconciliation);

        if ($failed_reconciliation === self::RECONCILIATION_EMPTY) {
            echo "ici";
        }
    }

    /** @param-out Reconciler::RECONCILIATION_* $f */
    public static function boo(
        ?int &$f = self::RECONCILIATION_OK
    ): void {
        $f = self::RECONCILIATION_EMPTY;
    }
}
Reconciler::reconcileKeyedTypes();
