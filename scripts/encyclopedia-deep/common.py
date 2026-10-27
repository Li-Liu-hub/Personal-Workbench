"""Shared constructors for deep-reading topics and upgrade topics."""

TOPIC = "八股题"


def topic(slug, language, category, title, summary, syntax, example, notes, source,
          tags, prompt, answer, rubric, training, card_title=None):
    return {
        "slug": slug,
        "language": language,
        "category": category,
        "title": title,
        "summary": summary,
        "syntax": syntax,
        "example": example,
        "notes": notes,
        "source": source,
        "tags": list(tags),
        "card": {
            "title": card_title or f"{title} · 精读推演",
            "topic": TOPIC,
            "prompt": prompt,
            "answer": answer,
            "rubric": list(rubric),
            "training": dict(training),
        },
    }


def upgrade(entry_id, summary, syntax, example, notes, source, tags,
            card_title, prompt, answer, rubric, training):
    return {
        "entry_id": entry_id,
        "card_id": f"knowledge:{entry_id}",
        "summary": summary,
        "syntax": syntax,
        "example": example,
        "notes": notes,
        "source": source,
        "tags": list(tags),
        "card": {
            "title": card_title,
            "topic": TOPIC,
            "prompt": prompt,
            "answer": answer,
            "rubric": list(rubric),
            "training": dict(training),
        },
    }
