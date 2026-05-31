<?php

class Reconciler
{
    public const RECONCILIATION_OK = 0;
    public const RECONCILIATION_REDUNDANT = 1;
    public const RECONCILIATION_EMPTY = 2;
}

class AssertionReconciler
{
    /**
     * @param Reconciler::RECONCILIATION_* $failed_reconciliation
     * @param-out Reconciler::RECONCILIATION_* $failed_reconciliation
     */
    public function reconcile(int &$failed_reconciliation = Reconciler::RECONCILIATION_EMPTY): void {}
}
