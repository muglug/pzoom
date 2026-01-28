<?php
class Assessment {
    private ?string $root = null;

    /** @psalm-mutation-free */
    public function getRoot(): ?string {
        return $this->root;
    }
}

class Project {
    private ?Assessment $assessment = null;

    /** @psalm-mutation-free */
    public function getAssessment(): ?Assessment {
        return $this->assessment;
    }
}

function f(Project $project): int {
    if (($project->getAssessment() === null)
        || ($project->getAssessment()->getRoot() === null)
    ) {
        throw new RuntimeException();
    }

    return strlen($project->getAssessment()->getRoot());
}