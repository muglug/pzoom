<?php
/** @var string $domainCandidate */;

$candidateLabels = explode('.', $domainCandidate);

$lastLabel = $candidateLabels[0];

if (strlen($lastLabel) === 2) {
    exit;
}
