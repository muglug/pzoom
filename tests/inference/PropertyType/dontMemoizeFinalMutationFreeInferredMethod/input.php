<?php
final class ExecutionMode
{
    private bool $isAutoCommitEnabled = true;

    public function enableAutoCommit(): void
    {
        $this->isAutoCommitEnabled = true;
    }

    public function disableAutoCommit(): void
    {
        $this->isAutoCommitEnabled = false;
    }

    public function isAutoCommitEnabled(): bool
    {
        return $this->isAutoCommitEnabled;
    }
}

$mode = new ExecutionMode();
$mode->disableAutoCommit();
assert($mode->isAutoCommitEnabled() === false);

$mode->enableAutoCommit();
assert($mode->isAutoCommitEnabled() === true);
