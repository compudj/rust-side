/*
 * What a program's libside sections cost it: how much address space
 * they take, how much of it this process ever fetched, and how much of
 * that it dirtied.
 *
 * Preloaded rather than linked into the programs it measures, because
 * those same binaries are what the section sizes are read from:
 * anything added to them would be counted as instrumentation.
 *
 * Residency comes from /proc/self/pagemap and not from mincore(),
 * which for a file backed mapping answers whether the page is in the
 * page cache -- true of a file just built, whether or not this process
 * ever touched it.
 *
 * Two moments matter, and this object holds both. A constructor which
 * runs before the program's own clears the soft dirty bits, so that
 * what is reported as dirtied is what this process wrote and not what
 * some earlier owner of the page did. A destructor reports, which is
 * after the constructors have registered the events and after a tracer
 * preloaded alongside has read every description.
 *
 * Environment:
 *
 *   PROBE_REPORT   file to append the report to. Standard error when
 *                  unset, which is where it stays out of the way of a
 *                  console tracer writing to standard output.
 */

#define _GNU_SOURCE
#include <elf.h>
#include <fcntl.h>
#include <link.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>
#include <unistd.h>

#define PAGE_SIZE	4096
/* Set in a pagemap entry when the page is in this process's tables. */
#define PAGEMAP_PRESENT	(1ULL << 63)
/* Set when the page was written since the soft dirty bits were cleared. */
#define PAGEMAP_DIRTY	(1ULL << 55)

static const char *const sections[] = {
	"side_event_description",
	"side_event_state",
	"side_event_state_ptr",
};

/* Where the loader put the executable, which is what sh_addr is relative to. */
static ElfW(Addr) bias;

static int take_bias(struct dl_phdr_info *info, size_t size, void *data)
{
	(void) size;
	(void) data;
	/* The executable is the first, and the only one with no name. */
	bias = info->dlpi_addr;
	return 1;
}

__attribute__((constructor(101)))
static void clear_soft_dirty(void)
{
	int fd = open("/proc/self/clear_refs", O_WRONLY);

	if (fd >= 0) {
		(void) !write(fd, "4\n", 2);
		close(fd);
	}
}

/* The pages of one range this process holds, and how many it wrote. */
static void count_pages(unsigned long start, unsigned long end,
			long *total, long *resident, long *dirty)
{
	unsigned long page;
	int fd;

	*total = *resident = *dirty = 0;
	fd = open("/proc/self/pagemap", O_RDONLY);
	if (fd < 0)
		return;
	for (page = start / PAGE_SIZE; page <= (end - 1) / PAGE_SIZE; page++) {
		uint64_t entry;

		if (pread(fd, &entry, sizeof(entry), page * sizeof(entry))
				!= sizeof(entry))
			break;
		(*total)++;
		if (entry & PAGEMAP_PRESENT)
			(*resident)++;
		if (entry & PAGEMAP_DIRTY)
			(*dirty)++;
	}
	close(fd);
}

/* Where a named section of the running executable begins and ends. */
static int find_section(const char *name, unsigned long *start, unsigned long *end)
{
	ElfW(Ehdr) header;
	ElfW(Shdr) shstr;
	char names[65536];
	int fd, found = 0;
	unsigned i;

	fd = open("/proc/self/exe", O_RDONLY);
	if (fd < 0)
		return 0;
	if (pread(fd, &header, sizeof(header), 0) != sizeof(header))
		goto out;
	if (pread(fd, &shstr, sizeof(shstr),
			header.e_shoff + header.e_shstrndx * sizeof(shstr))
			!= sizeof(shstr))
		goto out;
	if (shstr.sh_size > sizeof(names))
		goto out;
	if (pread(fd, names, shstr.sh_size, shstr.sh_offset)
			!= (ssize_t) shstr.sh_size)
		goto out;

	for (i = 0; i < header.e_shnum; i++) {
		ElfW(Shdr) section;

		if (pread(fd, &section, sizeof(section),
				header.e_shoff + i * sizeof(section))
				!= sizeof(section))
			break;
		if (section.sh_name >= shstr.sh_size)
			continue;
		if (strcmp(names + section.sh_name, name) != 0)
			continue;
		*start = bias + section.sh_addr;
		*end = *start + section.sh_size;
		found = section.sh_size != 0;
		break;
	}
out:
	close(fd);
	return found;
}

static long status_field(const char *key)
{
	char line[256];
	FILE *status;
	long value = 0;

	status = fopen("/proc/self/status", "re");
	if (!status)
		return 0;
	while (fgets(line, sizeof(line), status)) {
		if (strncmp(line, key, strlen(key)) == 0) {
			sscanf(line + strlen(key), "%ld", &value);
			break;
		}
	}
	fclose(status);
	return value;
}

__attribute__((destructor))
static void report(void)
{
	const char *path = getenv("PROBE_REPORT");
	FILE *out = stderr;
	unsigned i;

	dl_iterate_phdr(take_bias, NULL);

	if (path) {
		out = fopen(path, "ae");
		if (!out)
			out = stderr;
	}

	for (i = 0; i < sizeof(sections) / sizeof(sections[0]); i++) {
		unsigned long start, end;
		long total, resident, dirty;

		if (!find_section(sections[i], &start, &end)) {
			fprintf(out, "%-24s %10s\n", sections[i], "absent");
			continue;
		}
		count_pages(start, end, &total, &resident, &dirty);
		fprintf(out, "%-24s %10lu bytes %5ld pages %5ld resident %5ld dirty\n",
			sections[i], end - start, total, resident, dirty);
	}
	fprintf(out, "%-24s %10ld kB rss %26ld kB peak\n", "process",
		status_field("VmRSS:"), status_field("VmHWM:"));

	if (out != stderr)
		fclose(out);
}
