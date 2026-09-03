package org.bukkit.inventory.meta;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;

/** The mutable book metadata stored by Foton's API-side ItemStack. */
public final class SimpleBookMeta extends SimpleItemMeta implements WritableBookMeta {
    private static final int MAX_TITLE_LENGTH = 32;

    private String title;
    private String author;
    private List<String> pages = new ArrayList<>();
    private Generation generation;

    @Override
    public boolean hasTitle() {
        return title != null;
    }

    @Override
    public String getTitle() {
        return title;
    }

    @Override
    public boolean setTitle(String value) {
        if (value != null && value.length() > MAX_TITLE_LENGTH) {
            return false;
        }
        title = value;
        return true;
    }

    @Override
    public boolean hasAuthor() {
        return author != null;
    }

    @Override
    public String getAuthor() {
        return author;
    }

    @Override
    public void setAuthor(String value) {
        author = value;
    }

    @Override
    public boolean hasPages() {
        return !pages.isEmpty();
    }

    @Override
    public String getPage(int page) {
        return pages.get(index(page));
    }

    @Override
    public void setPage(int page, String data) {
        pages.set(index(page), data == null ? "" : data);
    }

    @Override
    public List<String> getPages() {
        return List.copyOf(pages);
    }

    @Override
    public void setPages(List<String> value) {
        pages = clean(value);
    }

    @Override
    public void setPages(String... value) {
        setPages(value == null ? null : Arrays.asList(value));
    }

    @Override
    public void addPage(String... added) {
        if (added == null) {
            return;
        }
        for (String page : added) {
            pages.add(page == null ? "" : page);
        }
    }

    @Override
    public int getPageCount() {
        return pages.size();
    }

    @Override
    public Generation getGeneration() {
        return generation;
    }

    @Override
    public void setGeneration(Generation value) {
        generation = value;
    }

        @Override
    public SimpleBookMeta clone() {
        SimpleBookMeta copy = (SimpleBookMeta) super.clone();
        copy.pages = new ArrayList<>(pages);
        return copy;
    }

    @Override
    public boolean equals(Object other) {
        return super.equals(other)
            && other instanceof SimpleBookMeta book
            && java.util.Objects.equals(title, book.title)
            && java.util.Objects.equals(author, book.author)
            && pages.equals(book.pages)
            && generation == book.generation;
    }

    @Override
    public int hashCode() {
        return java.util.Objects.hash(super.hashCode(), title, author, pages, generation);
    }

    private int index(int page) {
        if (page < 1 || page > pages.size()) {
            throw new IllegalArgumentException(
                "page must be between 1 and " + pages.size() + ", got " + page);
        }
        return page - 1;
    }

    private static List<String> clean(List<String> value) {
        List<String> answer = new ArrayList<>();
        if (value != null) {
            for (String page : value) {
                answer.add(page == null ? "" : page);
            }
        }
        return answer;
    }
}
